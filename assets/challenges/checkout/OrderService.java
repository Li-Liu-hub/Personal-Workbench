package training;
import java.math.BigDecimal;
/** Checkout entry point. Keep public method signatures compatible with existing callers. */
public final class OrderService {
    private final Catalog catalog;
    private final Inventory inventory;
    private final OrderRepository orders;
    public OrderService(Catalog catalog, Inventory inventory, OrderRepository orders) {
        this.catalog = catalog;
        this.inventory = inventory;
        this.orders = orders;
    }
    public Order place(String requestId, String sku, int quantity) {
        BigDecimal price = catalog.price(sku);
        inventory.reserve(sku, quantity);
        Order existing = orders.find(requestId);
        if (existing != null) return existing;
        if (inventory.available(sku) < 0) throw new IllegalStateException("insufficient stock");
        Order order = new Order(requestId, sku, quantity, price.multiply(BigDecimal.valueOf(quantity)));
        orders.save(order);
        return order;
    }
}
