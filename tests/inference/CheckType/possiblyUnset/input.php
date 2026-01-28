<?php
try {
    $foo = 1;
} catch (Exception $_) {
}
/** @psalm-check-type $foo = 1 */;
            
