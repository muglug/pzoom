<?php
class One {
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
        if ($one !== null && ($two || rand(0, 1))) {
            $one->fooFoo();
        }
    }
}