# Licensed CC-0-1.0 in the Catalog

FROM archlinux/base

RUN pacman -Syuq --noconfirm git base-devel sudo

# Skip the "great powers... great responsibilities" pedantry
RUN    echo "Defaults        lecture = never" > /etc/sudoers.d/privacy \
    && echo "%wheel ALL=(ALL) NOPASSWD: ALL" > /etc/sudoers.d/wheel

ENV ARCH_FRONTEND=noninteractive

# Circle CI dependences
RUN pacman -S --noconfirm openssh mercurial xorg-server-xvfb \
    ca-certificates tar gzip parallel \
    net-tools netcat unzip zip bzip2 gnupg curl wget

# timezone
RUN ln -sf /usr/share/zoneinfo/UTC /etc/localtime

# Use unicode
RUN locale-gen C.UTF-8 || true
ENV LANG=C.UTF-8
    
RUN JQ_URL="https://circle-downloads.s3.amazonaws.com/circleci-images/cache/linux-amd64/jq-latest" \
  && curl --silent --show-error --location --fail --retry 3 --output /usr/bin/jq $JQ_URL \
  && chmod +x /usr/bin/jq \
  && jq --version

# Install Docker & CI Goodies

# Docker.com returns the URL of the latest binary when you hit a directory listing
# We curl this URL and `grep` the version out.
# The output looks like this:

#>    # To install, run the following commands as root:
#>    curl -fsSLO https://download.docker.com/linux/static/stable/x86_64/docker-17.05.0-ce.tgz && tar --strip-components=1 -xvzf docker-17.05.0-ce.tgz -C /usr/local/bin
#>
#>    # Then start docker in daemon mode:
#>    /usr/local/bin/dockerd

RUN set -ex \
  && export DOCKER_VERSION=$(curl --silent --fail --retry 3 https://download.docker.com/linux/static/stable/x86_64/ | grep -o -e 'docker-[.0-9]*\.tgz' | sort -r | head -n 1) \
  && DOCKER_URL="https://download.docker.com/linux/static/stable/x86_64/${DOCKER_VERSION}" \
  && echo Docker URL: $DOCKER_URL \
  && curl --silent --show-error --location --fail --retry 3 --output /tmp/docker.tgz "${DOCKER_URL}" \
  && ls -lha /tmp/docker.tgz \
  && tar -xz -C /tmp -f /tmp/docker.tgz \
  && mv /tmp/docker/* /usr/bin \
  && rm -rf /tmp/docker /tmp/docker.tgz \
  && which docker \
  && (docker version || true)

# docker compose
RUN COMPOSE_URL="https://circle-downloads.s3.amazonaws.com/circleci-images/cache/linux-amd64/docker-compose-latest" \
  && curl --silent --show-error --location --fail --retry 3 --output /usr/bin/docker-compose $COMPOSE_URL \
  && chmod +x /usr/bin/docker-compose \
  && docker-compose version

# install dockerize
RUN DOCKERIZE_URL="https://circle-downloads.s3.amazonaws.com/circleci-images/cache/linux-amd64/dockerize-latest.tar.gz" \
  && curl --silent --show-error --location --fail --retry 3 --output /tmp/dockerize-linux-amd64.tar.gz $DOCKERIZE_URL \
  && tar -C /usr/local/bin -xzvf /tmp/dockerize-linux-amd64.tar.gz \
  && rm -rf /tmp/dockerize-linux-amd64.tar.gz \
  && dockerize --version

RUN groupadd --gid 3434 circleci \
  && useradd --uid 3434 -G wheel --gid circleci --shell /bin/bash --create-home circleci \
  && echo 'circleci ALL=NOPASSWD: ALL' >> /etc/sudoers.d/50-circleci \
  && echo 'Defaults    env_keep += "ARCH_FRONTEND"' >> /etc/sudoers.d/env_keep


# Prettiness
RUN pacman -S --noconfirm archey3

# Vulkano dependencies
RUN pacman -Qkk --noconfirm gcc glibc
RUN pacman -S --noconfirm gcc7 cmake python

# Shaderc dependencies
RUN pacman -S --noconfirm shaderc gcc-libs asciidoctor \
    cmake make glibc glslang spirv-tools

# Rust dependencies
RUN pacman -S --noconfirm unzip zlib binutils llvm gcc

# Clear the package cache for teeny tiny image
RUN pacman -Syu --noconfirm
RUN pacman -Scc --noconfirm


USER circleci
CMD ["/bin/sh"]
# Now commands run as user `circleci`

# Switching user can confuse Docker's idea of $HOME, so we set it explicitly
ENV HOME /home/circleci

# Prettiness
RUN echo "archey3" >> ~/.bashrc

# Installing rustfmt & toolchain to home directories
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
ENV LD_LIBRARY_PATH=$LD_LIBRARY_PATH:/usr/local/lib
ENV PATH=$PATH:/home/circleci/.cargo/bin/
RUN echo "export PATH=$PATH:/home/circleci/.cargo/bin/" >> ~/.bashrc
RUN rustup toolchain install stable
RUN rustup component add rustfmt
