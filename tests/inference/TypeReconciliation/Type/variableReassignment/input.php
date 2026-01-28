<?php
class One {
    /** @return void */
    public function fooFoo() {}
}

class Two {
    /** @return void */
    public function barBar() {}
}

$one = new One();

$one = new Two();

$one->barBar();