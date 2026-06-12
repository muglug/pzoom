<?php

function reportMissingClass(string $message): void {
    if (preg_match('/^Could not get class storage for (.*)$/', $message, $matches)
        && !class_exists($matches[1])
        && !interface_exists($matches[1])
        && !enum_exists($matches[1])
    ) {
        echo "Class used in CallMap does not exist: {$matches[1]}";
    }
}

function reversedOrder(string $name): void {
    if (!interface_exists($name) && !class_exists($name) && !trait_exists($name)) {
        echo "nothing named {$name}";
    }
}
