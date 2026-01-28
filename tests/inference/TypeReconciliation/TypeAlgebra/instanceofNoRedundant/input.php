<?php
function logic(Foo $a, Foo $b) : void {
    if ((!$a instanceof Bat || !$b instanceof Bat)
        && (!$a instanceof Bat || !$b instanceof Bar)
        && (!$a instanceof Bar || !$b instanceof Bat)
        && (!$a instanceof Bar || !$b instanceof Bar)
    ) {

    } else {
        if ($b instanceof Bat) {}
    }
}

class Foo {}
class Bar extends Foo {}
class Bat extends Foo {}