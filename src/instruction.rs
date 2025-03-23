use borsh::BorshDeserialize;

use solana_program::program_error::ProgramError;

pub enum MovieInstruction {
    AddMovieReview {
        title: String,
        rating: u8,
        description: String,
    },
    UpdateMovieReview {
        title: String,
        rating: u8,
        description: String,
    },
    AddComment {
        comment: String,
    },
}

impl MovieInstruction {
    pub fn unpack(input: &[u8]) -> Result<Self, ProgramError> {
        let (&discriminator, rest) = input.split_first()
            .ok_or(ProgramError::InvalidInstructionData)?;

        Ok(match discriminator {
            0 => {
                let payload = MovieReviewPayload::try_from_slice(rest)
                    .map_err(|_| ProgramError::InvalidInstructionData)?;

                Self::AddMovieReview { 
                    title: payload.title, 
                    rating: payload.rating, 
                    description: payload.description 
                }
            },
            1 => {
                let payload = MovieReviewPayload::try_from_slice(rest)
                    .map_err(|_| ProgramError::InvalidInstructionData)?;

                Self::UpdateMovieReview { 
                    title: payload.title, 
                    rating: payload.rating, 
                    description: payload.description 
                }
            },
            2 => {
                let payload = CommentPayload::try_from_slice(rest)
                    .map_err(|_| ProgramError::InvalidInstructionData)?;

                Self::AddComment { 
                    comment: payload.comment 
                }
            },

            _ => return Err(ProgramError::InvalidInstructionData)
        })
    }
}

#[derive(BorshDeserialize)]
struct MovieReviewPayload {
    title: String,
    rating: u8,
    description: String,
}

#[derive(BorshDeserialize)]
struct CommentPayload {
    comment: String,
}
