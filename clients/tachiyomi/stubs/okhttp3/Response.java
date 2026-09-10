package okhttp3;

public class Response implements java.io.Closeable {
    public ResponseBody body() {
        return null;
    }

    public Request request() {
        return null;
    }

    @Override
    public void close() {}
}
