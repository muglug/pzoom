<?php
class One {
    /** @return void */
    public function fooFoo() {}
}

class B {
    /** @return void */
    public function barBar(One $one = null) {
        if ($one === null) {
            $one = new One();
        }

        $one->fooFoo();
    }
}