<?php
class One {
    /** @return void */
    public function fooFoo() {}
}

class B {
    /** @return void */
    public function barBar(One $one = null) {
        if ($one) {
            // do nothing
        }
        else {
            $one = new One();
        }

        $one->fooFoo();
    }
}