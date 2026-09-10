package okhttp3;

public class Request {
    public HttpUrl url() {
        return null;
    }

    public static class Builder {
        public Builder url(String url) {
            return this;
        }

        public Builder headers(Headers headers) {
            return this;
        }

        public Request build() {
            return new Request();
        }
    }
}
