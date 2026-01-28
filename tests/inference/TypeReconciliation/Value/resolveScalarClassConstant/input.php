<?php
class PaymentFailure {
    const NO_CLIENT = "no_client";
    const NO_CARD = "no_card";
}

/**
 * @return PaymentFailure::NO_CARD|PaymentFailure::NO_CLIENT
 */
function something() {
    if (rand(0, 1)) {
        return PaymentFailure::NO_CARD;
    }

    return PaymentFailure::NO_CLIENT;
}

function blah(): void {
    $test = something();
    if ($test === PaymentFailure::NO_CLIENT) {}
}