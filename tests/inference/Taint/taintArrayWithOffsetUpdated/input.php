<?php
function foo() {
    $foo = [
        "a" => [["c" => "hello"]],
        "b" => [],
    ];

    $foo["b"][] = [
        "c" => $_GET["bad"],
    ];

    bar($foo["b"]);
}

function bar(array $arr): void {
    foreach ($arr as $s) {
        echo $s["c"];
    }
}
