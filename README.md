# version-rs
[![CircleCI](https://circleci.com/gh/clux/version-rs/tree/master.svg?style=shield)](https://circleci.com/gh/clux/version-rs/tree/master)
[![docker pulls](https://img.shields.io/docker/pulls/clux/version.svg)](
https://hub.docker.com/r/clux/version/)
[![docker image info](https://images.microbadger.com/badges/image/clux/version.svg)](http://microbadger.com/images/clux/version)
[![docker tag](https://images.microbadger.com/badges/version/clux/version.svg)](https://hub.docker.com/r/clux/version/tags/)

An example kube deployment reflector and actix web server in ~100 lines of rust. It exposes a simple version api for deployments on `/versions`.

## Usage
Connect to a kube cluster and give it a namespace to watch for deployments:

```sh
$ NAMESPACE=dev cargo run
```

then you can get simplified version info back on its web server:

```sh
$ curl localhost:8000/versions
[{"container":"quay.io/babylonhealth/raftcat","name":"raftcat","version":"0.112.0"}]

$ curl localhost:8000/versions/raftcat
{"container":"quay.io/babylonhealth/raftcat","name":"raftcat","version":"0.112.0"}
```

and its metrics:

```sh
$ curl localhost:8000/metrics
api_http_requests_duration_seconds_bucket{endpoint="/",method="GET",status="200",le="0.005"} 11
...
...
api_http_requests_duration_seconds_bucket{endpoint="/",method="GET",status="200",le="+Inf"} 11
api_http_requests_duration_seconds_sum{endpoint="/",method="GET",status="200"} 0.001559851
api_http_requests_duration_seconds_count{endpoint="/",method="GET",status="200"} 11
# HELP api_http_requests_total Total number of HTTP requests
# TYPE api_http_requests_total counter
api_http_requests_total{endpoint="/",method="GET",status="200"} 11
```

## Deploying to kubernetes
Start a minikube cluster and apply the [deployment.yaml](./deployment.yaml) in the default namespace.

Then hit the service's cluster ip with our url:

```sh
curl "$(kubectl get service  -oyaml version | yq .spec.clusterIP -r)/versions/version"
```
