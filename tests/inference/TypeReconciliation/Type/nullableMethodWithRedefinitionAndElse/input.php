<?php
class One {
    /** @var int|null */
    public $two;

    /** @return void */
    public function fooFoo() {}
}

class B {
    /** @return void */
    public function barBar(One $one = null) {
        if (!$one) {
            $one = new One();
        }
        else {
            $one->two = 3;
        }

        $one->fooFoo();
    }
}