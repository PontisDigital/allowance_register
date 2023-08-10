#!/bin/bash
cargo lambda build --release
cp target/lambda/allowance_sign_up/bootstrap bootstrap && zip lambda.zip bootstrap twilio_auth.json firebase_auth.json && rm bootstrap
