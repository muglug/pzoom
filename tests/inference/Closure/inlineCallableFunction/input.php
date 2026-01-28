<?php
class A {
    function bar(): void {
        function foobar(int $a, int $b): int {
            return $a > $b ? 1 : 0;
        }

        $arr = [5, 4, 3, 1, 2];

        usort($arr, "fooBar");
    }
}
