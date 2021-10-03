docker_build('clux/version', '.', dockerfile='Dockerfile')
k8s_yaml('deployment.yaml')
k8s_resource('version', port_forwards=8000)
