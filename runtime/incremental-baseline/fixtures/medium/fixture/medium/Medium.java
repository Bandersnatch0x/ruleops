package fixture.medium;

public final class Medium {
    private Medium() {}

    public static int value() {
        return Helper.value() + Model.value();
    }
}
