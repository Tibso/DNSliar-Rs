use crate::handler_mod::CustomError;

use trust_dns_client::{
    op::LowerQuery,
    rr::{RecordType, RData}
};
use trust_dns_proto::rr::Record;
use trust_dns_resolver::{
    config::{ResolverConfig, ResolverOpts, NameServerConfig, Protocol},
    Name,
    TokioAsyncResolver,
    AsyncResolver,
    name_server::{GenericConnection, GenericConnectionProvider, TokioRuntime}
};
use std::{
    str::FromStr,
    net::SocketAddr
};

pub fn build_resolver (
    sockets: Vec<SocketAddr>
)
-> AsyncResolver<GenericConnection, GenericConnectionProvider<TokioRuntime>> {
    let mut resolver_config = ResolverConfig::new();
    resolver_config.domain();

    for socket in sockets {
        let ns_udp = NameServerConfig::new(socket, Protocol::Udp);
        resolver_config.add_name_server(ns_udp);
        let ns_tcp = NameServerConfig::new(socket, Protocol::Tcp);
        resolver_config.add_name_server(ns_tcp)
    }
    
    let resolver = TokioAsyncResolver::tokio(
        resolver_config,
        ResolverOpts::default()
    ).unwrap();

    println!("Resolver built");
    return resolver
}

pub async fn get_answers (
    request: &LowerQuery,
    resolver: AsyncResolver<GenericConnection, GenericConnectionProvider<TokioRuntime>>
)
-> Result<Vec<Record>, CustomError> {    
    let mut answers: Vec<Record> =  Vec::new();
    let name_binding = request.name().to_string();
    let name = name_binding.as_str();
    match request.query_type() {
        RecordType::A => {
            let response = match resolver.ipv4_lookup(name).await {
                Ok(ok) => ok,
                Err(error) => return Err(CustomError::ResolverError(error))
            };

            for rdata in response {
                answers.push(Record::from_rdata(Name::from_str(name).unwrap(), 60, RData::A(rdata)));
            }
        },
        RecordType::AAAA => {
            let response = match resolver.ipv6_lookup(name).await {
                Ok(ok) => ok,
                Err(error) => return Err(CustomError::ResolverError(error))
            };

            for rdata in response {
                answers.push(Record::from_rdata(Name::from_str(name).unwrap(), 60, RData::AAAA(rdata)));
            } 
        },
        RecordType::TXT => {
            let response = match resolver.txt_lookup(name).await {
                Ok(ok) => ok,
                Err(error) => return Err(CustomError::ResolverError(error))
            };

            for rdata in response {
                answers.push(Record::from_rdata(Name::from_str(name).unwrap(), 60, RData::TXT(rdata)));
            } 
        },
        RecordType::SRV => {
            let response = match resolver.srv_lookup(name).await {
                Ok(ok) => ok,
                Err(error) => return Err(CustomError::ResolverError(error))
            };

            for rdata in response {
                answers.push(Record::from_rdata(Name::from_str(name).unwrap(), 60, RData::SRV(rdata)));
            }
        },
        _ => todo!()
    }

    return Ok(answers)
}