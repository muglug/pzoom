<?php
class A {
    /** @return array<A|null> */
    public static function loadMultiple()
    {
        return [new A, null];
    }

    /** @return void */
    public function barBar() {

    }
}

foreach (A::loadMultiple() as $a) {
    if ($a === null) {
        // do nothing
    }

    $a->barBar();
}
