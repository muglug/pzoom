<?php
class A {
    /** @return true */
    public function returnsTrue() { return true; }

    /** @return false */
    public function returnsFalse() { return false; }

    /** @return bool */
    public function returnsBool() {
        if (rand() % 2 > 0) {
            return true;
        }
        return false;
    }
}