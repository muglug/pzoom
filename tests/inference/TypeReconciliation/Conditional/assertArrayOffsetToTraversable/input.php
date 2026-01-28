<?php
function render(array $data): ?Traversable {
    if ($data["o"] instanceof Traversable) {
        return $data["o"];
    }

    return null;
}