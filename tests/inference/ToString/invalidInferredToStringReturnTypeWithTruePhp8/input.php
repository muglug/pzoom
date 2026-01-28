<?php
class A {
    function __toString() {
        /** @psalm-suppress InvalidReturnStatement */
        return true;
    }
}
