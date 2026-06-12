<?php
/** @template T */
interface Message {}

/** @implements Message<int> */
final class FirstMessage implements Message {}

/** @implements Message<int> */
final class SecondMessage implements Message {}

/**
 * @template T
 * @param Message<T> $msg
 */
function test(Message $msg): void {}

/** @var FirstMessage|SecondMessage $message */;
test($message);
