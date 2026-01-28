<?php
$a = null;
$b = &$a;

try {
    throw new \Exception();
} catch (\Exception $a) {
    takesException($b);
}
function takesException(\Exception $e): void {}
