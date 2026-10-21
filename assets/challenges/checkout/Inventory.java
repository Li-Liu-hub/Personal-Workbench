package training;
import java.util.HashMap;
import java.util.Map;
public final class Inventory {
    private final Map<String, Integer> amounts = new HashMap<>();
    public void seed(String sku, int amount) { amounts.put(sku, amount); }
    public int available(String sku) { return amounts.getOrDefault(sku, 0); }
    public void reserve(String sku, int quantity) { amounts.put(sku, available(sku) - quantity); }
}
