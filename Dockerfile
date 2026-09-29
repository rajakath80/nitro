# nitro-dev/Dockerfile
FROM amazonlinux:2

# 1) System tools + Nitro CLI dependencies
RUN yum -y update && \
    yum install -y \
      gcc gcc-c++ make cmake git jq bc \
      util-linux-user gettext-devel \
    && amazon-linux-extras enable aws-nitro-enclaves-cli \
    && yum install -y aws-nitro-enclaves-cli

# 2) Install Rust toolchain
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /workspace