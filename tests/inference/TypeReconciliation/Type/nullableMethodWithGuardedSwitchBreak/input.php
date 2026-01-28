<?php
class One {
    /** @return void */
    public function fooFoo() {}
}

class B {
    /** @return void */
    public function barBar(One $one = null) {
        $a = 4;

        switch ($a) {
            case 4:
                if ($one === null) {
                    break;
                }

                $one->fooFoo();
                break;
        }
    }
}
