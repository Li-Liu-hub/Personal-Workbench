package training;
import java.math.BigDecimal;
public record Order(String requestId, String sku, int quantity, BigDecimal total) {}
