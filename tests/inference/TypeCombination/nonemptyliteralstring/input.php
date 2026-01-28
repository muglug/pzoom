<?php
/** @var non-empty-literal-string */
$instance = "test";
$provider = "test";
$key = "";
if (random_int(0, 1)) {
    $key = "{$instance}_{$provider}";
    /** @psalm-check-type-exact $key=non-empty-literal-string */;
}
/** @psalm-check-type-exact $key=literal-string */;

$_ = $key !== "" ? "test{$key}]" : "test";
                
