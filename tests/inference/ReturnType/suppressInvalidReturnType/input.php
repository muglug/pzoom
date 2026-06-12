<?php
/**
 * @psalm-suppress InvalidReturnType
 */
function calculate(string $foo): int {
    switch ($foo) {
        case "a":
            return 0;
    }
}
