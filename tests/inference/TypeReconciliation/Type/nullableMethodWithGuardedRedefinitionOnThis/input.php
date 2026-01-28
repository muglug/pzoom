<?php
class One {
    /** @return void */
    public function fooFoo() {}
}

class B {
    /** @var One|null */
    public $one;

    /** @return void */
    public function barBar(One $one = null) {
        $this->one = $one;

        if ($this->one === null) {
            $this->one = new One();
        }

        $this->one->fooFoo();
    }
}