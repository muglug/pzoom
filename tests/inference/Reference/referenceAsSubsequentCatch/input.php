<?php
$a = null;
$b = &$a;

try {
    throw new \Exception();
} catch (\Exception $b) {
    takesException($a);
}
function takesException(\Exception $e): void {}
