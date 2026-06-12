<?php
class A {
    /**
     * @return bool
     */
    public function fooFoo() {
        switch (rand(0,10)) {
            case 1:
            case 2:
                return true;

            default:
                throw new \Exception("badness");
        }
    }
}
