FROM gcr.io/distroless/static:nonroot
COPY --chown=nonroot:nonroot ./version /app/
EXPOSE 8080
ENTRYPOINT ["/app/version"]
