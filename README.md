# version-rs

An example kube deployment reflector and actix web server in ~100 lines of rust. It exposes a simple version api for deployments on `/`.

## Usage
Connect to a kube cluster and give it a namespace to watch for deployments:

```sh
$ NAMESPACE=dev cargo run
```

then you can get simplified version info back on its web server:

```sh
$ curl localhost:8000/
[{"container":"quay.io/babylonhealth/raftcat","name":"raftcat","version":"0.112.0"}]
```
