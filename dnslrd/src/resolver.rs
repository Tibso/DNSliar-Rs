use crate::{
    CONFILE,
    structs::{Config, DnsLrResult, DnsLrError, DnsLrErrorKind, ExternCrateErrorKind}
};

use trust_dns_client::{
    op::ResponseCode,
    rr::RecordType,
};
use trust_dns_proto::rr::Record;
use trust_dns_resolver::{
    config::{ResolverConfig, ResolverOpts, NameServerConfig, Protocol},
    TokioAsyncResolver,
    AsyncResolver,
    name_server::{GenericConnection, GenericConnectionProvider, TokioRuntime},
    IntoName,
    error::{ResolveErrorKind, ResolveError},
    lookup::Lookup
};
use trust_dns_server::server::Request;

use tracing::info;

/// Builds the resolver that will forward the requests to other DNS servers
pub fn build_resolver (
    config: &Config
)
-> AsyncResolver<GenericConnection, GenericConnectionProvider<TokioRuntime>> {
    // Resolver's configuration variable is initialized
    let mut resolver_config = ResolverConfig::new();
    // Resolver's domain is set to the local domain
    resolver_config.domain();

    // The forwarders' sockets are cloned out of the configuration variable
    // They are then made into an iterable to iterate onto
    for socket in config.forwarders.clone().into_iter() {
        // Both UDP and TCP are configured for each socket
        let ns_udp = NameServerConfig::new(socket, Protocol::Udp);
        resolver_config.add_name_server(ns_udp);
        let ns_tcp = NameServerConfig::new(socket, Protocol::Tcp);
        resolver_config.add_name_server(ns_tcp);
    }
    
    // Default values of the resolver are used
    let mut resolver_opts: ResolverOpts = ResolverOpts::default();
    // We do not want the resolver to send concurrent queries,
    // as it would increase network load for little to no speed benefit
    resolver_opts.num_concurrent_reqs = 0;
    // Resolver is built
    let resolver = TokioAsyncResolver::tokio(
        resolver_config,
        resolver_opts
    ).unwrap();

    info!("{}: Resolver built", CONFILE.daemon_id);
    resolver
}

/// Uses the resolver to retrieve the correct answers
pub async fn get_answers (
    request: &Request,
    resolver: AsyncResolver<GenericConnection, GenericConnectionProvider<TokioRuntime>>
)
-> DnsLrResult<Vec<Record>> {
    // Answers vector is initialized to be pushed into later
    let mut answers: Vec<Record> =  vec![];
    // The domain name of the request is converted to string
    let name = request.query().name().into_name().unwrap();

    // The result variable of the resolver queries is defined here to increase its scope,
    // so all the results can be handled later
    let wrapped: Result<Lookup, ResolveError>;
    // Each query_type is handled here for the resolver
    match request.query().query_type() {
        RecordType::A => wrapped = resolver.lookup(name, RecordType::A).await,
        RecordType::AAAA => wrapped = resolver.lookup(name, RecordType::AAAA).await,
        RecordType::TXT => wrapped = resolver.lookup(name, RecordType::TXT).await,
        RecordType::SRV => wrapped = resolver.lookup(name, RecordType::SRV).await,
        RecordType::MX => wrapped = resolver.lookup(name, RecordType::MX).await,
        RecordType::PTR => {
            // PTR queries results need to be handled separetely,
            // as the result is of a different type

            // ArpaAddress is parsed, if it is invalid,
            // the appropriate error is propagated up in the stack
            let Ok(ip) = name.parse_arpa_name() else {
                return Err(DnsLrError::from(DnsLrErrorKind::InvalidArpaAddress))
            };
            
            // Subnet address is converted to an IP
            let ip = ip.addr();
            return match resolver.reverse_lookup(ip).await {
                Ok(ok) => {
                    for record in ok.as_lookup().records() {
                        answers.push(record.clone())
                    }
                    Ok(answers)
                },
                Err(err) => {
                    match err.kind() {
                        ResolveErrorKind::NoRecordsFound {response_code: ResponseCode::Refused, ..}
                            => Err(DnsLrError::from(DnsLrErrorKind::RequestRefused)),
                        ResolveErrorKind::NoRecordsFound {..}
                            => Ok(vec![]),
                        _ => Err(DnsLrError::from(DnsLrErrorKind::ExternCrateError(ExternCrateErrorKind::ResolverError(err))))
                    }
                }
            }
        },
        _ => return Ok(vec![])
    };

    // The result of the resolver queries are handled here
    match wrapped {
        // If no error occured
        Ok(ok) => {
            // Answers the resolver received are cloned in the new answers
            for record in ok.records() {
                answers.push(record.clone())
            }
            Ok(answers)
        },
        // If an error occured
        Err(err) => {
            // Error types are handled differently
            match err.kind() {
                // If the resolver's query was refused,
                // propagate the appropriate error up in the stack
                ResolveErrorKind::NoRecordsFound {response_code: ResponseCode::Refused, ..}
                    => Err(DnsLrError::from(DnsLrErrorKind::RequestRefused)),
                // If no record was found, creates an empty answer
                ResolveErrorKind::NoRecordsFound {..}
                    => Ok(vec![]),
                // If another error type occured, propagate it up in the stack
                _ => Err(DnsLrError::from(DnsLrErrorKind::ExternCrateError(ExternCrateErrorKind::ResolverError(err))))
            }
        }
    }
}