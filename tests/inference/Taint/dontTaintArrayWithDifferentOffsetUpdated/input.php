<?php
function foo(): void {
    $foo = [
        "a" => [["c" => "hello"]],
        "b" => [],
    ];

    $foo["b"][] = [
        "c" => $_GET["bad"],
    ];

    bar($foo["a"]);
}

function bar(array $arr): void {
    foreach ($arr as $s) {
        echo $s["c"];
    }
}
