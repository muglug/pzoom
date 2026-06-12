<?php
$r = false;
/** @var object $o */;
/** @var string $m */;
if (method_exists($o, $m)) {
    $r = $o->$m(...);
}
