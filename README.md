# version-rs
[![CircleCI](https://circleci.com/gh/kube-rs/version-rs/tree/master.svg?style=shield)](https://circleci.com/gh/kube-rs/version-rs/tree/master)
[![docker image](https://img.shields.io/docker/pulls/clux/version.svg)](
https://hub.docker.com/r/clux/version/tags/)

An example kube deployment reflector and actix web server in ~100 lines of rust. It exposes a simple version api for deployments on `/versions`.

## Usage
Start the watcher against your current kubernetes context:

```sh
cargo run
```

This will monitor the namespace of your context, and give you simplified version info on its web server:

```sh
$ curl 0.0.0.0:8000/versions
[{"container":"clux/controller","name":"foo-controller","version":"latest"},{"container":"alpine","name":"debugger","version":"3.13"}]

$ curl 0.0.0.0:8000/versions/default/foo-controller
{"container":"clux/controller","name":"foo-controller","version":"latest"}
```

and its metrics (currently disabled due to actix upgrade issues):

```sh
$ curl 0.0.0.0:8000/metrics
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

## Developing
- Locally against a cluster: `cargo run`
- In-cluster: edit and `[tilt](https://tilt.dev/) up`
