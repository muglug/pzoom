<?php
function a(): ?string
{
    return rand(0, 10) ? "elePHPant" : null;
}

$a = a();
