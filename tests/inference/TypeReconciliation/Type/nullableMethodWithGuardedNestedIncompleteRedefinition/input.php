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
        $a = rand(0, 4);

        if ($one === null) {
            if ($a === 4) {
                $one = new One();
            }
        }

        $one->fooFoo();
    }
}
