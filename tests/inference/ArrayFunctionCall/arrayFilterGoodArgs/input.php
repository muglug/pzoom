<?php
function fooFoo(int $i) : bool {
  return true;
}

class A {
    public static function barBar(int $i) : bool {
        return true;
    }
}

array_filter([1, 2, 3], "fooFoo");
array_filter([1, 2, 3], "foofoo");
array_filter([1, 2, 3], "FOOFOO");
array_filter([1, 2, 3], "A::barBar");
array_filter([1, 2, 3], "A::BARBAR");
array_filter([1, 2, 3], "A::barbar");
