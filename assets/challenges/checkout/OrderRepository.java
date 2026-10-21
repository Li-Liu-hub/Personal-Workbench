package training;
import java.util.HashMap;
import java.util.Map;
public final class OrderRepository {
    private final Map<String, Order> orders = new HashMap<>();
    public Order find(String requestId) { return orders.get(requestId); }
    public void save(Order order) { orders.put(order.requestId(), order); }
    public int size() { return orders.size(); }
}
