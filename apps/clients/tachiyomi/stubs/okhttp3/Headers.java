package okhttp3;

public class Headers {
    public static class Builder {
        public Builder set(String name, String value) {
            return this;
        }

        public Builder add(String name, String value) {
            return this;
        }

        public Headers build() {
            return new Headers();
        }
    }
}
