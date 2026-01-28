<?php
class One {
    /** @return void */
    public function fooFoo() {}
}

class B {
    /** @return void */
    public function barBar(One $one) {
        if (!$one) {
            // do nothing
        }

        $one->fooFoo();
    }
}
