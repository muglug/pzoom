<?php
function g(SimpleXMLElement $e): void {
    foreach ($e->directory as $directory) {
        echo (string) $directory['name'];
    }
    foreach ($e as $child) {
        echo $child->getName();
    }
}
