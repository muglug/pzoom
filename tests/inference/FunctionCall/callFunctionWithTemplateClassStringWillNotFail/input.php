<?php
/** @param class-string<SplFixedArray<string>> $classString */
function acceptTemplatedClassString(string $classString): void
{
}

/** @param class-string<SplFixedArray<string>> $classString */
function app(string $classString): void
{
    acceptTemplatedClassString($classString);
}
