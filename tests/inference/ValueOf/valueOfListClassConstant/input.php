<?php
class A {
    const FOO = [
        "bar"
    ];
    /** @return value-of<A::FOO> */
    public function getKey() {
        return "bar";
    }
}
