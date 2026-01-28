<?php
class A {
    const FOO = [
        "bar" => 42
    ];
    /** @return value-of<A::FOO> */
    public function getValue() {
        return 42;
    }
}
