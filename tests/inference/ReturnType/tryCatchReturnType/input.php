<?php
class A {
    /** @return bool */
    public function fooFoo() {
        try {
            // do a thing
            return true;
        }
        catch (\Exception $e) {
            throw $e;
        }
    }
}
