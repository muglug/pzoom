<?php
$a = rand(0, 1) ? (function(): void {}) : 1;
if (!is_callable($a)) {
    echo $a;
}