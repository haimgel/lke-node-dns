#!/bin/sh

docker build -t node-dns .
docker run -it --rm \
  --env LINODE_API_TOKEN=$LINODE_API_TOKEN \
  --env NODE_DOMAIN=$NODE_DOMAIN \
  --mount type=bind,source=~/.kube/config,target=/home/app/.kube/config \
  node-dns
