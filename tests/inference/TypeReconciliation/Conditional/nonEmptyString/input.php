<?php
/**
 * @psalm-param non-empty-string $name
 */
function sayHello(string $name) : void {
    echo "Hello " . $name;
}

function takeInput() : void {
    if (isset($_GET["name"]) && is_string($_GET["name"])) {
        $name = trim($_GET["name"]);
        sayHello($name);
    }
}
