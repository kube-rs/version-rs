docker_build('ghcr.io/kube-rs/version-rs:local', '.', dockerfile='Dockerfile')
local_resource('import', 'just import')
k8s_yaml('deployment.yaml')
k8s_resource('version', port_forwards=8000)
