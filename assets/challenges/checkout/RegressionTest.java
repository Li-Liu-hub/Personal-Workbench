package training;
import java.math.BigDecimal;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicInteger;
public final class RegressionTest {
    record Fixture(OrderService service, Inventory inventory, OrderRepository orders) {}
    static Fixture fixture(int stock) {
        Catalog catalog = new Catalog(); catalog.add("BOOK", "19.90"); catalog.add("PEN", "3.50");
        Inventory inventory = new Inventory(); inventory.seed("BOOK", stock); inventory.seed("PEN", 10);
        OrderRepository orders = new OrderRepository();
        return new Fixture(new OrderService(catalog, inventory, orders), inventory, orders);
    }
    static void check(boolean valid, String message) { if (!valid) throw new AssertionError(message); }
    static void rejected(Runnable call) {
        try { call.run(); } catch (IllegalArgumentException | IllegalStateException expected) { return; }
        throw new AssertionError("request should have been rejected");
    }
    static void parallel(int n, Runnable call) throws Exception {
        ExecutorService pool = Executors.newFixedThreadPool(n);
        CountDownLatch start = new CountDownLatch(1);
        for (int i=0;i<n;i++) pool.submit(() -> { try { start.await(); call.run(); } catch (InterruptedException e) { Thread.currentThread().interrupt(); } });
        start.countDown(); pool.shutdown();
        check(pool.awaitTermination(4,TimeUnit.SECONDS), "workers did not finish");
    }
    interface Test { void run() throws Exception; }
    public static void main(String[] args) {
        String[] names={"normal-order", "duplicate-retry", "zero-quantity", "negative-quantity", "insufficient-stock", "request-id-conflict", "unknown-sku", "parallel-orders", "parallel-retries"};
        Test[] tests={
            () -> { Fixture f=fixture(10); Order o=f.service.place("r1","BOOK",2); check(o.total().compareTo(new BigDecimal("39.80"))==0,"wrong total");check(f.inventory.available("BOOK")==8,"wrong stock"); },
            () -> { Fixture f=fixture(10);Order a=f.service.place("r1","BOOK",2);Order b=f.service.place("r1","BOOK",2);check(a.equals(b)&&f.inventory.available("BOOK")==8&&f.orders.size()==1,"retry reserved twice"); },
            () -> { Fixture f=fixture(10);rejected(()->f.service.place("r1","BOOK",0));check(f.inventory.available("BOOK")==10&&f.orders.size()==0,"invalid request changed data"); },
            () -> { Fixture f=fixture(10);rejected(()->f.service.place("r1","BOOK",-1));check(f.inventory.available("BOOK")==10,"negative quantity changed stock"); },
            () -> { Fixture f=fixture(10);rejected(()->f.service.place("r1","BOOK",11));check(f.inventory.available("BOOK")==10&&f.orders.size()==0,"failed order changed stock"); },
            () -> { Fixture f=fixture(10);f.service.place("r1","BOOK",2);rejected(()->f.service.place("r1","PEN",1));check(f.inventory.available("PEN")==10&&f.inventory.available("BOOK")==8,"id conflict changed stock"); },
            () -> { Fixture f=fixture(10);rejected(()->f.service.place("r1","UNKNOWN",1));check(f.orders.size()==0&&f.inventory.available("BOOK")==10,"unknown SKU changed data"); },
            () -> { Fixture f=fixture(10);AtomicInteger ids=new AtomicInteger();AtomicInteger successes=new AtomicInteger();parallel(20,()->{try{f.service.place("p"+ids.getAndIncrement(),"BOOK",1);successes.incrementAndGet();}catch(IllegalStateException expected){}});check(successes.get()==10&&f.orders.size()==10&&f.inventory.available("BOOK")==0,"parallel stock invariant broken"); },
            () -> { Fixture f=fixture(10);parallel(10,()->f.service.place("same","BOOK",1));check(f.inventory.available("BOOK")==9&&f.orders.size()==1,"parallel retry reserved more than once"); }
        };
        int passed=0;
        for (int i=0;i<tests.length;i++) {
            try {tests[i].run();passed++;System.out.println("PASS "+names[i]);}
            catch(Throwable failure) {System.out.println("FAIL "+names[i]+" : "+failure.getMessage());}
        }
        System.out.println("TOTAL "+passed+"/"+tests.length);
        if(passed!=tests.length) System.exit(1);
    }
}
