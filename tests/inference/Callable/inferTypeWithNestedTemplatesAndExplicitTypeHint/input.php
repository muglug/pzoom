<?php
/**
 * @template TResult
 */
interface Message {}

/**
 * @implements Message<list<int>>
 */
final class GetListOfNumbers implements Message {}

/**
 * @template TResult
 * @template TMessage of Message<TResult>
 */
final class Envelope {}

/**
 * @template TResult
 * @template TMessage of Message<TResult>
 * @param class-string<TMessage> $_message
 * @param callable(TMessage, Envelope<TResult, TMessage>): TResult $_handler
 */
function addHandler(string $_message, callable $_handler): void {}

addHandler(GetListOfNumbers::class, function (Message $_message, Envelope $_envelope) {
    /**
     * @psalm-check-type-exact $_message = GetListOfNumbers
     * @psalm-check-type-exact $_envelope = Envelope<list<int>, GetListOfNumbers>
     */
    return [1, 2, 3];
});
