package training;
import java.math.BigDecimal;
import java.util.HashMap;
import java.util.Map;
public final class Catalog {
    private final Map<String, BigDecimal> prices = new HashMap<>();
    public void add(String sku, String price) { prices.put(sku, new BigDecimal(price)); }
    public BigDecimal price(String sku) {
        BigDecimal price = prices.get(sku);
        if (price == null) throw new IllegalArgumentException("unknown SKU");
        return price;
    }
}
