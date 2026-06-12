<?php
class A {
    /** @return bool */
    public function fooFoo() {
        switch (rand(0,10)) {
            case 1:
                $a = 5;
            default:
                return true;
        }
    }
}
