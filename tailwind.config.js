/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      fontFamily: {
        sf: ['"SF Pro Display"', '"Segoe UI Variable"', "system-ui", "sans-serif"],
      },
      backdropBlur: {
        glass: "30px",
      },
      boxShadow: {
        glass: "0 10px 40px rgba(0,0,0,0.15)",
      },
      borderRadius: {
        glass: "20px",
      },
      animation: {
        "pulse-recording": "pulse-recording 1.5s ease-in-out infinite",
        "waveform": "waveform 0.8s ease-in-out infinite",
      },
      keyframes: {
        "pulse-recording": {
          "0%, 100%": { opacity: "1" },
          "50%": { opacity: "0.4" },
        },
        waveform: {
          "0%, 100%": { transform: "scaleY(0.3)" },
          "50%": { transform: "scaleY(1)" },
        },
      },
    },
  },
  plugins: [],
};
