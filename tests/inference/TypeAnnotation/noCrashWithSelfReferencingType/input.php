<?php
/**
 * @psalm-type SomeType = array{
 *     parent?: SomeType,
 *     foo?: int,
 * }
 * @psalm-param SomeType $input
 */
function test(array $input):void {}
