<?php
namespace A {
    $a = "1";
}
namespace B\C {
    $bc = "2";
}
namespace {
    echo $a . PHP_EOL;
    echo $bc . PHP_EOL;
}
