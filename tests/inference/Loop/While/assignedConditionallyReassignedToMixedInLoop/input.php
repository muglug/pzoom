<?php
function foo(array $arr): void {
    while (rand(0, 1)) {
        $t = true;
        if (!empty($arr[0])) {
            /** @psalm-suppress MixedAssignment */
            $t = $arr[0];
        }
        if ($t === true) {}
    }
}
