<?php
                    namespace A {
                        /**
                         * @psalm-internal A
                         */
                        trait T {
                            public static function barBar(): void {
                            }
                        }

                        class Foo {
                            use T;

                        }
                    }

                    namespace B {
                        class Bat {
                            public function batBat() : void {
                                \A\Foo::barBar();
                            }
                        }
                    }
