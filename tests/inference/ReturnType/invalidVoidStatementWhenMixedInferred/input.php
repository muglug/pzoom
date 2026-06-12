<?php
/**
 * @return mixed
 */
function a()
{
    return 1;
}

function b(): void
{
    return a();
}
