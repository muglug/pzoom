<?php
/**
 * @psalm-pure
 */
function getException(): \Exception
{
    return new \Exception();
}

throw getException();
