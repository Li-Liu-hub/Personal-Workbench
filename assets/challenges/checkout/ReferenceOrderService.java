package training;
import java.math.BigDecimal;
public final class OrderService {
    private final Catalog catalog;
    private final Inventory inventory;
    private final OrderRepository orders;
    public OrderService(Catalog catalog, Inventory inventory, OrderRepository orders) {
        this.catalog = catalog; this.inventory = inventory; this.orders = orders;
    }
    public synchronized Order place(String requestId, String sku, int quantity) {
        if (requestId == null || requestId.isBlank() || quantity <= 0)
            throw new IllegalArgumentException("invalid request");
        Order existing = orders.find(requestId);
        if (existing != null) {
            if (!existing.sku().equals(sku) || existing.quantity() != quantity)
                throw new IllegalArgumentException("request id reused for a different order");
            return existing;
        }
        BigDecimal price = catalog.price(sku);
        if (inventory.available(sku) < quantity) throw new IllegalStateException("insufficient stock");
        inventory.reserve(sku, quantity);
        Order order = new Order(requestId, sku, quantity, price.multiply(BigDecimal.valueOf(quantity)));
        orders.save(order);
        return order;
    }
}
