<?php
/** @psalm-type TA = array<int, string> */

class Bar {
    public function foo() : void {
        $bar =
            /** @return TA */
            function() {
                return ["hello"];
        };

        /** @var array<int, TA> */
        $bat = [$bar(), $bar()];

        foreach ($bat as $b) {
            echo $b[0];
        }
    }
}

/**
 * @psalm-type _A=array{elt:int}
 * @param _A $p
 * @return _A
 */
function f($p) {
    /**
     * @psalm-suppress UnnecessaryVarAnnotation
     * @var _A
     */
    $r = $p;
    return $r;
}
