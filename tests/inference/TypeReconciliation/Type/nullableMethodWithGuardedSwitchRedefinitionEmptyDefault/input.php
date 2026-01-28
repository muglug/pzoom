<?php
class One {
    /** @return void */
    public function fooFoo() {}
}

class B {
    /** @return void */
    public function barBar(One $one = null) {
        $a = rand(0, 4);

        if ($one === null) {
            switch ($a) {
                case 4:
                    $one = new One();
                    break;

                default:
                    break;
            }
        }

        $one->fooFoo();
    }
}
