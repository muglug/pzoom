<?php
/**
 * @template T
 */
class Box {}

/**
 * @param Box<contravariant string> $box
 */
function consume(Box $box): void {}
