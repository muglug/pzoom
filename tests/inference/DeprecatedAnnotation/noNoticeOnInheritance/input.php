<?php
/**
 * @deprecated
 */
class Foo {}

interface Iface {
    /**
     * @psalm-suppress DeprecatedClass
     * @return Foo[]
     */
    public function getFoos();

    /**
     * @psalm-suppress DeprecatedClass
     * @return Foo[]
     */
    public function getDifferentFoos();
}

class Impl implements Iface {
    public function getFoos(): array {
        return [];
    }

    public function getDifferentFoos() {
        return [];
    }
}
