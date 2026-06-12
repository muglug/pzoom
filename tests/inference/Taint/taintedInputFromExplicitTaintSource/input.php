<?php
/**
 * @psalm-taint-source input
 */
function getName() : string {
    return "";
}

echo getName();
