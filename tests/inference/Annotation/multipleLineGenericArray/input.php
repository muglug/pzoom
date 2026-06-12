<?php
/**
 * @psalm-type MiddlewareArray = array<
 *     class-string<\Exception>,
 *     array<int, string>
 * >
 *
 * @psalm-type RuleArray = array{
 *     rule: string,
 *     controller?: class-string<\Exception>,
 *     redirect?: string,
 *     code?: int,
 *     type?: string,
 *     middleware?: MiddlewareArray
 * }
 *
 * Foo Bar
 */
class A {}
