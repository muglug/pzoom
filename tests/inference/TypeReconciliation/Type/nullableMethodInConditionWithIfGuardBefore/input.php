<?php
class One {
    /** @var string */
    public $a = "";

    /** @return void */
    public function fooFoo() {}
}

class Two {
    /** @return void */
    public function fooFoo() {}
}

class B {
    /** @return void */
    public function barBar(One $one = null, Two $two = null) {
        if ($one === null) {
            return;
        }

        if (!$one->a && $one->fooFoo()) {
            // do something
        }
    }
}