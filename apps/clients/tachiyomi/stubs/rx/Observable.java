package rx;

import java.util.concurrent.Callable;

public class Observable<T> {
    public static <T> Observable<T> fromCallable(Callable<? extends T> callable) {
        return new Observable<T>();
    }
}
