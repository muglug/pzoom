<?php
namespace A;

class DeliveryTimeAggregated {}

/**
 * @template T of object
 */
interface ReplayMessageInterface
{
    /**
     * @return class-string<T>
     */
    public static function messageName(): string;
}

/**
 * @template-implements ReplayMessageInterface<DeliveryTimeAggregated>
 */
class ReplayDeliveryTimeAggregated implements ReplayMessageInterface
{
    /**
     * @return class-string<DeliveryTimeAggregated>
     */
    public static function messageName(): string
    {
        return DeliveryTimeAggregated::class;
    }
}