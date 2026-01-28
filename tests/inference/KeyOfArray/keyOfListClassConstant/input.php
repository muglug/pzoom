<?php
class A {
    const FOO = [
        "bar"
    ];
    /** @return key-of<A::FOO> */
    public function getKey() {
        return 0;
    }
}
