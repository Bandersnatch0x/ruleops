package fixture.large;

public final class Large {
    private Large() {}

    public static int value() {
        return A.value() + B.value() + C.value() + D.value();
    }
}
