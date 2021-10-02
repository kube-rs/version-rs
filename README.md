# version-rs
[![CircleCI](https://circleci.com/gh/kube-rs/version-rs/tree/master.svg?style=shield)](https://circleci.com/gh/kube-rs/version-rs/tree/master)
[![docker image](https://img.shields.io/docker/pulls/clux/version.svg)](
https://hub.docker.com/r/clux/version/tags/)

An example kube deployment reflector and actix web server in ~100 lines of rust. It exposes a simple version api for deployments on `/versions`.

## Usage
Clone the repo and either run locally or deploy into a cluster:

### Locally
Run against your current kubernetes context:

```sh
cargo run
```

### In-Cluster
Apply [deployment.yaml](./deployment.yaml), then `kubectl port-forward service/version 8000:8000`

### Api
Once running, the app will monitor the namespace of your context, and give you simplified version info on its web interface:

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

## Developing
- Locally against a cluster: `cargo run`
- In-cluster: edit and `tilt up` [*](https://tilt.dev/)

To build the image directly, run:

```sh
DOCKER_BUILDKIT=1 docker build -t clux/version .
```
